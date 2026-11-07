<schema xmlns="http://purl.oclc.org/dsdl/schematron" xmlns:xsl="http://www.w3.org/1999/XSL/Transform" queryBinding="xslt2">
  <title>Authored preservation fixture</title>
  <ns prefix="t" uri="urn:fixture"/>
  <ns prefix="u" uri="urn:fixture:helpers"/>
  <let name="value" value="'global'"/>
  <let name="forward" value="$later"/>
  <phase id="chosen"><active pattern="second"/><active pattern="first"/></phase>
  <phase id="other"><active pattern="inactive"/></phase>
  <xsl:function name="u:recursive" as="xs:integer">
    <xsl:param name="value" as="xs:integer"/>
    <xsl:choose>
      <xsl:when test="$value = 0"><xsl:sequence select="0"/></xsl:when>
      <xsl:otherwise>
        <xsl:variable name="local" select="$value - 1"/>
        <xsl:variable name="text">00001 padded </xsl:variable>
        <xsl:sequence select="u:recursive($local)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>
  <xsl:function name="u:unused" as="xs:string"><xsl:sequence select="$later"/></xsl:function>
  <include href="common.sch"/>
  <pattern id="first">
    <rule context="t:item[@code = $value]">
      <let name="value" value="concat($value, '-local')"/>
      <assert id="one" flag="fatal" test="every $value in ('01', ' 002 ') satisfies (some $inner in ($value) satisfies $inner != '')">before <name/> at <value-of select="$value"/> tail</assert>
    </rule>
    <rule context="t:*"><assert id="overlap" flag="warning" test="true()">overlap</assert></rule>
  </pattern>
  <pattern id="second"><rule context="t:item"><assert id="independent" flag="information" test="for $i in 1 to 2, $j in $i return u:recursive($j)">independent</assert></rule></pattern>
  <pattern id="inactive"><rule context="//t:item[$later]"><assert id="not-selected" flag="fatal" test="false()">Retain me</assert></rule></pattern>
</schema>
